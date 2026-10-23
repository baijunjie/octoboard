# 09 Use shared neutral empty presentations

> Goal: use shared neutral empty presentations.
> Completion criteria: The directory-picker empty listing uses the shared compact presentation and ordinary unavailable-agent empty states use a neutral/purpose glyph. All listed validation passes, and the independently reviewed result is accepted.

Status: **accepted; implementation not started**. Implementation begins only after the audit passes and the preceding required work is closed.

## Scope

The accepted findings are reproduced below so this brief remains self-contained.

- **Directory picker (S03):** `DirectoryPicker.tsx` currently uses a disabled list option with fading text only.
  Keep that valid option and collection semantics, but give the empty listing the shared compact glyph/message
  presentation. Preserve parent navigation, keyboard skipping and the absence of row actions. The existing
  `dialog-directory-picker` only shows populated content; add an empty fixture and compare populated, parent,
  root and failed-list states.
- **No installed agent (S05/G03):** `TerminalPane.tsx` and `Sidebar.tsx` use a warning triangle for ordinary
  absence, while the added-but-not-installed state is different. Use a neutral or purpose glyph in these two
  empty states; retain PATH explanations, disabled creation actions, genuine load failures and the approved
  warning glyph in `AgentTabList`.
- **Integrated history handoff:** re-inventory `SessionHistoryView`'s `history-unavailable` and `history-failed`
  Alerts against "Where an error is shown"; both are on-screen failures with no error dialog, and `history-failed`
  keeps its reason, details, retry and reconnect behavior. These callers arrived after the original audit baseline.

## Implementation contract

- [ ] Give the empty directory listing the shared compact glyph/message shape within valid collection markup; preserve disabled option semantics and parent navigation.
- [ ] Replace warning imagery only in ordinary terminal/sidebar no-installed-agent absence; retain PATH explanations and disabled creation actions.
- [ ] Preserve intentional warning glyph on AgentTabList, true failed-load error states, shared full-panel headings and silent/action exclusivity.

## Validation requirements

- [ ] Add or settle an empty-directory fixture and compare populated/parent/root/failed-list states. Review empty-console-no-agent against empty-console-no-agent-added in both themes and Chinese/ar. Verify no empty row action, keyboard skipping, valid list semantics, no duplicate creation entry and matching sidebar/main shapes. Run affected gallery and dialog/empty-panel tests and static checks.
- [ ] Complete the project gallery/static/change-check/documentation gates; preserve failed or unresolved checks explicitly. Do not skip scenarios or weaken assertions.

## Dependencies and handoff

None; no token redesign or unrelated empty-state copy changes.

## Test invocation

Use `pnpm --filter @octoboard/ui exec vitest run <explicit paths>` for focused UI tests. Passing a path after
`pnpm ... test --` did not constrain the audit's run and launched the wider suite. Record the actual command,
selected files and outcome; do not infer a whole-suite pass from a focused check.

## Notes for the developer

Reuse the existing shared components and mechanisms named in the linked finding record. Follow `docs/ui-accessibility.md`, `packages/ui/README.md` “The UI state gallery”, `docs/voiceover-cases.md`, and project gallery/change-check workflows. Only gallery fixtures may stage data. Read the exact baseline evidence and recheck caller inventory after rebasing; other owners’ pending changes are not certified by this audit.

## Integrated archived-history handoff

Local main added history states at `501bd872d`, after the historical audit. The chip, badge and error presentation
change has since made both history failures on-screen Alerts with no error dialog: `history-unavailable` (the record
could not be read, then the remedy) and `history-failed` (the rejected request with its reason, details and Try
again, announced once). Re-inventory them against "Where an error is shown" in `docs/ui-accessibility.md`; preserve
retry and reconnect re-reading, and refresh normal-absence severity decisions here without treating the earlier audit
as certification.
