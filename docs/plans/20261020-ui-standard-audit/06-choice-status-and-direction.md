# 06 Separate choice status and sentence values

> Goal: separate choice status and sentence values.
> Completion criteria: Status follows the name as a badge; technical/named values in prose have the right rich treatment and every string substitution has an appropriate bidi boundary. All listed validation passes, and the independently reviewed result is accepted.

Status: **accepted; implementation not started**. Implementation begins only after the audit passes and the preceding required work is closed.

## Scope

COPY-04, COPY-05, numeric-ratio half of COPY-06 and COPY-08 in [the copy audit](audit-copy.md).

## Implementation contract

- [ ] Move Add agent statuses, missing launch model/effort values and gone-branch status into the supported badge display path, retaining raw collection labels/textValue.
- [ ] Isolate inherited-account/model strings, import labels, tag removal and fallback errors only at sentence boundaries; keep sort/match/key/default submitted values unmodified.
- [ ] Render the comparing base branch as a technical inherit chip and its announcement as isolated text; keep the primary branch selector structured.
- [ ] Mark the drawn count/limit ratio LTR and preserve ordinary translated prose direction. Preserve the approved AgentTabList warning glyph and plain accessibility labels.

## Validation requirements

- [ ] Review every COPY-04/05/06/08 trigger and popup with long names, absent stored values, select clearing, type-to-select, arrow selection and dialog Enter submission. Include Add agent all statuses, gone branch, pending comparison, inherited account/model and import labels. Actual Chinese/ar/light/dark captures must show badge placement and punctuation order; run catalog/select/branch tests, chip geometry, full gallery for shared choice changes and static checks.
- [ ] Complete the project gallery/static/change-check/documentation gates; preserve failed or unresolved checks explicitly. Do not skip scenarios or weaken assertions.

## Dependencies and handoff

Milestone 05 established `Option.labelContent` for rich display and overflow tooltips, while `label` stays raw for
collection and native matching. `TextInput.labelContent` pairs rich visible copy with its plain `label`;
`TextInput.errorDetails` and `OptionSelect.descriptionDetails` retain external supplementary content. Reuse these
contracts for this milestone's existing choice/status/direction findings, with one owner for further shared API changes.

Resume-by-ID is integrated on main at `368a1f1f3`; no separate resume branch is pending. Refresh the choice and
sentence inventory against `ResumeConversationFields`, `useResumeTarget` and `useLaunchChoice`: a located record
fixes its agent/account in disabled single-option fields, while lookup status uses a separate plain announcement.
Preserve raw option values and matching text, the fixed holder, and the existing isolation of referenced names;
do not turn a disabled holder into an editable fallback. Exercise `dialog-new-session-resume-record`,
`dialog-new-session-resume-record-account` and the archived/interrupted/live variants when shared choice contracts
change. The original `3a65c903` audit does not certify these added routes.

## Review and regression-test handoff

The change-check brief must explicitly include **names referenced in visible copy use the shared InlineChip**,
including fixed agent/product names where they are references in prose. Review rich visible surfaces separately
from plain accessible/native/matching surfaces. Drive manual source closure from the complete inventory; do not
add a broad lint/static-scanner framework. Add focused regression tests for the shared rich/plain parameter
contract, and retain the shared stock Alert contract tests. Avoid tests that only restate a caller's markup.

## Test invocation

Use `pnpm --filter @octoboard/ui exec vitest run <explicit paths>` for focused UI tests. Passing a path after
`pnpm ... test --` did not constrain the audit's run and launched the wider suite. Record the actual command,
selected files and outcome; do not infer a whole-suite pass from a focused check.

## Notes for the developer

Reuse the existing shared components and mechanisms named in the linked finding record. Follow `docs/ui-accessibility.md`, `packages/ui/README.md` “The UI state gallery”, `docs/voiceover-cases.md`, and project gallery/change-check workflows. Only gallery fixtures may stage data. Read the exact baseline evidence and recheck caller inventory after rebasing; other owners’ pending changes are not certified by this audit.
