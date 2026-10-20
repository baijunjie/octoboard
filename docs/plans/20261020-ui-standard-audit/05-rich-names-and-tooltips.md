# 05 Use chips for names referenced in visible copy

> Goal: use chips for names referenced in visible copy.
> Completion criteria: Visible prose, titles, control references and rich tooltip content render names in inherit-size nonmonospace chips and technical literals in their supported technical form; plain surfaces stay readable text. All listed validation passes, and the independently reviewed result is accepted.

Status: **accepted; implementation not started**. Implementation begins only after the audit passes and the preceding required work is closed.

## Scope

COPY-02, COPY-03 and the application-caller half of COPY-09 in [the copy audit](audit-copy.md).

## Implementation contract

- [ ] Cover every render route listed in COPY-02/03/09 and the interpolation inventory, including fixed agent names used as references, owner titles, restore titles, metadata tooltips, login/trust/request and import fallback copy.
- [ ] Separate rich visible tooltip/title/description values from plain accessible names, notifications, native labels, matching textValue and collection keys.
- [ ] Retain whole structured entity/path rows and source/terminal/report content in their established layouts; use the actual render semantics rather than a product-name exclusion.
- [ ] Preserve per-label hover scope, tooltip descriptions, portal control fills, wrapping, bidi isolation and shared size roles.

## Validation requirements

- [ ] Use the exact scenario/fixture-gap matrix in COPY-02/03/09. Add missing named-error/tooltip cases through gallery-writer; verify all affected surface kinds, long mixed-script values, both themes, Chinese and Arabic. Check names remain verbatim in accessible labels, matching has no isolates, tooltip content is rich, and focus/hover suppression still works. Run chip geometry, catalog tests, affected caller tests, shared-layer full gallery and static checks.
- [ ] Complete the project gallery/static/change-check/documentation gates; preserve failed or unresolved checks explicitly. Do not skip scenarios or weaken assertions.

## Dependencies and handoff

The common daemon renderer now resolves producer-declared record IDs separately from literal names, including
agent/product references, and supplies inline-safe summaries with external recovery and diagnostic details. Reuse
that rich/plain boundary; do not flatten its metadata or repeat its summary. Other application-caller names and
tooltips remain this delivery's work.

## Review and regression-test handoff

The change-check brief must explicitly include **names referenced in visible copy use the shared InlineChip**,
including fixed agent/product names where they are references in prose. Review rich visible surfaces separately
from plain accessible/native/matching surfaces. Drive manual source closure from the complete inventory; do not
add a broad lint/static-scanner framework. Add focused regression tests for the shared rich/plain parameter
contract, and retain the shared Alert contract tests. Avoid tests that only restate a caller's markup.

## Test invocation

Use `pnpm --filter @octoboard/ui exec vitest run <explicit paths>` for focused UI tests. Passing a path after
`pnpm ... test --` did not constrain the audit's run and launched the wider suite. Record the actual command,
selected files and outcome; do not infer a whole-suite pass from a focused check.

## Re-inventory after concurrent quota integration

The audit evidence is pinned to `3a65c903`. Local main subsequently integrated the 95-file quota change at
`7a7a2f34ebc19c5bb2ee29f9ae804354c2a40ad3`; original source line numbers, 425-message index and gallery counts do
not certify that code. Before implementation, refresh the relevant current source and fixture inventory, including
`useLaunchChoice` usedUp in session/handoff/switch-account, `dialogs/useQuickCreateSession.ts`, `quotaUsedUp` /
`quotaUsedUpAgent` recovery and refusal content. Recheck `quota/accountUsage.tsx` / ReasonText and `quota/quotaFormat.ts`;
`quotaUsedUpAlert` now supplies separate plain account-title and reason/reset-description values to
SessionDialog, HandoffDialog and SwitchAccountDialog. The visible title still references an isolated plain account
name; migrate it to the rich name contract without recolouring the whole description or changing the plain/native
sentence contract (`quotaUsedUpSentence`). Include ReasonText, accountUsage, quota recovery/refusal and import
fallback names in the same rich/plain review.

Validate the quota routes with `dialog-new-session-quota-all-used-up`,
`dialog-hand-off-quota-all-used-up`, `dialog-switch-account-quota-used-up`, `dialog-new-session-quota-refused` and
`sidebar-quick-create-quota-refused` in both themes and translated UI. Preserve the shared stock Alert composition. Structured recovery/error routing and visible referenced
account/agent names retain their separate semantic and rich-copy ownership. Preserve plain
accessible/native strings and announcements. The exact late-arriving inline Work target is `dialog-switch-account-quota-used-up`; the new-session refusal
also names Work in the acknowledgement dialog. The all-used-up new-session/handoff fixtures use Codex Default
(and Team), and quick-create is a local default-agent refusal. Do not describe all five as Work scenes.
Local main also integrated resume-by-ID at `368a1f1f3`; preserve its resume-aware `quotaUsedUp` gating.
Historical audit evidence does not certify that source change.

## Integrated resume-by-ID handoff

`useHeldElsewhere` now retains structured project/console holder reasons. The Session ID field renders the task,
session and holder names through directed chips, preserving the full task/name display; lookup and open-time
refusals share that path. Keep these field errors and their forced holder choices intact. Review other visible
caller prose separately from the plain `resumeFound*` status announcements, held-option labels and matching
`textValue`. `ResumeConversationFields` and the current `dialog-new-session-resume-*` fixtures are newer than the
historical audit, which does not certify them.

## Current rendering boundary

- `DaemonErrorMessage` without `part` renders the complete message; `part="summary"` is safe inside a stock
  Alert.Description, field error or sentence; `part="details"` supplies the remaining recovery/list/diagnostic
  content once outside it. `TextInput.errorDetails` and `OptionSelect.descriptionDetails` carry such external
  field content. Preserve the association of the short reason and the field.
- `DiagnosticBlock` keeps confirmed raw CLI/Git output LTR, unwrapped and keyboard-scrollable. A missing record
  ID is a technical chip; a producer-supplied name remains a name even when it happens to equal another record ID.
- `quotaUsedUpAlert` deliberately still returns plain account-title and reason/reset-description strings, and
  `quotaUsedUpSentence` remains the plain/native sentence API. The remaining visible account-name migration must
  preserve those APIs, late announcements and the existing resume-aware gating.
- Quick-create's exhausted-agent refusal retains its action metadata through `quick_create_quota_refused`;
  the unrelated owner-gone caller still needs the visible-name review listed in COPY-02. Model request failures
  and history failures now retain semantic metadata instead of becoming generic strings/states.

The current source adds direct known-name/ID-fallback, schema-field, nested diagnostic and raw shell-timeout
fixtures, plus the acknowledged and pending `history-failed` targets. Re-inventory the actual catalog rather than
reusing historical scenario counts. Keep `useErrorRecords` usable before a daemon provider exists, as startup
acknowledgements also render shared error copy.

## Notes for the developer

Reuse the existing shared components and mechanisms named in the linked finding record. Follow `docs/ui-accessibility.md`, `packages/ui/README.md` “The UI state gallery”, `docs/voiceover-cases.md`, and project gallery/change-check workflows. Only gallery fixtures may stage data. Read the exact baseline evidence and recheck caller inventory after rebasing; other owners’ pending changes are not certified by this audit.
