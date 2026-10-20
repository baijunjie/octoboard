# 07 Make form hints and blocked states accurate

> Goal: make form hints and blocked states accurate.
> Completion criteria: Every editable text input has an accurate required example or optional live default, confirmation literals keep their intended order, and fixed-owner invalidation closes the unusable form with an acknowledgement-only explanation. All listed validation passes, and the independently reviewed result is accepted.

Status: **accepted; implementation not started**. Implementation begins only after the audit passes and the preceding required work is closed.

## Scope

I-01, I-02, I-06 in [the interaction audit](audit-interactions.md) and S04 in [the state audit](audit-states.md).

## Implementation contract

- [ ] Complete the editable-input inventory: console/rename/path/typed confirmation and edit-project fields need placeholders even when prefilled.
- [ ] Prefix required path/URL/import-directory examples appropriately in every offered catalog; show actual directory/repository default for optional new project name and accurate empty-state semantics for tags.
- [ ] Give confirmation word and editable control explicit appropriate LTR while retaining ordinary italic UI font, localized word matching and input composition.
- [ ] Close a session dialog when its immutable fixed owner disappears; use the existing plain recovery-none error flow, keeping optional picked-owner fallback distinct and restoring focus.

## Validation requirements

- [ ] Exercise creation/edit/rename, directory navigation, typed deletion, optional tags and project source/name changes; check placeholders while empty and populated, requiredness and derived defaults. Update focus-console-new-session-owner-gone to the closed-form acknowledgement target and check focus, no recovery action, held-key suppression and keyboard submit. Inspect light/dark/Chinese/RTL and run affected dialog/field tests plus static/gallery checks.
- [ ] Complete the project gallery/static/change-check/documentation gates; preserve failed or unresolved checks explicitly. Do not skip scenarios or weaken assertions.

## Dependencies and handoff

S04 error presentation uses the shared stock Alert contract. Resume-by-ID is integrated on main at `368a1f1f3`;
no separate resume branch is pending. Re-inventory `SessionDialog`, `ResumeConversationFields`, `TextInput` and
`useResumeTarget` before changing form defaults or fixed-owner invalidation. The Resume tab initially shows only
Session ID; a located record adds an optional new-session Name and fixed agent/account, an archived session offers
an editable Name whose empty value retains its existing name, and interrupted/running sessions show a disabled
Name. Preserve field-level lookup/refusal feedback, pending-submit blocking, late status announcements, stale-answer
suppression and focus handoff to Session ID when dependent fields disappear or become disabled. Keep New-tab
picked-owner fallback distinct from fixed-owner closure. Check the resume-invalid, pending, record,
record-account, archived, interrupted, live, outside-project, outside-console, unlocated, unavailable,
lookup-failed and not-found fixtures alongside the existing owner-gone target. The original `3a65c903` audit does
not certify these added routes.

## Test invocation

Use `pnpm --filter @octoboard/ui exec vitest run <explicit paths>` for focused UI tests. Passing a path after
`pnpm ... test --` did not constrain the audit's run and launched the wider suite. Record the actual command,
selected files and outcome; do not infer a whole-suite pass from a focused check.

## Notes for the developer

Reuse the existing shared components and mechanisms named in the linked finding record. Follow `docs/ui-accessibility.md`, `packages/ui/README.md` “The UI state gallery”, `docs/voiceover-cases.md`, and project gallery/change-check workflows. Only gallery fixtures may stage data. Read the exact baseline evidence and recheck caller inventory after rebasing; other owners’ pending changes are not certified by this audit.
