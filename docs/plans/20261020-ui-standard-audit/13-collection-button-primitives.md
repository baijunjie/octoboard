# 13 Use the shared button primitive for collection chevrons

> Goal: use the shared button primitive for collection chevrons.
> Completion criteria: Files/Git tree chevrons follow the shared HeroUI Button contract, preserving React Aria collection slot, expansion semantics, row focus and equivalent row action. All listed validation passes and the result is independently reviewed.

Status: **paused after milestone 08**. The shared-button migration is implemented on its separate branch and
has its own initial validation, but final actual interaction coverage and closure remain incomplete. Do not
resume until the user clears the menu-focus scope decision and the coordinator resumes the audit after the
requested pause. The separate branch must not be integrated into this aggregate without explicit coordination.

## Scope

The accepted finding and its open scope decision are reproduced below so this brief remains self-contained.

- `treeRow.tsx` uses a bare React Aria Button for the chevron even though the installed HeroUI Button wraps the
  same primitive and forwards `slot`. Migrate the chevron to the shared Button without adding a Tab stop; retain
  collection-managed expansion/focus and the documented custom section-heading control.
- Existing Files traversal reached the folder action, returned to the row, expanded with Enter and moved to the
  next row with ArrowDown. The 16px glyph is not itself a pointer-target defect because the whole row is the
  equivalent larger action. Recheck Files and grouped Changes in en/ar light/dark plus Chinese, including arrows,
  Home/End, fold focus, menus, visible rings and the 24px equivalent action.
- **Held scope decision:** closing a keyboard-opened row menu returns focus to the row; project documentation
  promises return to the exact surviving Actions button. The same behavior occurs on the untouched baseline. The
  proposed bounded public-API repair uses a one-shot guarded focus return for the original connected row/key/tree
  and surviving trigger, leaving pointer behavior and virtualized stale targets to the collection. The user has
  not chosen whether to include that pre-existing defect in 13 or keep it as a separately tracked failing boundary.
  Keep 13 paused until that choice; do not weaken the exact-trigger assertion or infer consent from time.

## Implementation and validation contract

- [ ] Use the supported shared HeroUI button with the collection chevron slot, retaining compact layout and collection-managed focus without adding Tab stops.
- [ ] Preserve the documented custom section-heading band exception; its reason and successful heading focus probe do not justify every bare collection button.
- [ ] Verify whole-row Enter/Space expansion, chevron pointer expansion, arrow traversal, Home/End, fold focus and rows’ controls in en/ar light/dark, plus Chinese. Keep 24px equivalent row action; do not claim the drawn 16px glyph is itself a pointer failure.
- [ ] Run tree/collection focus tests, actual gallery traversal, affected gallery and static checks. If a real library constraint prevents the wrapper, document source evidence and agree the smallest justified exception rather than silently rewriting keyboard handling.
- [ ] Complete project gallery/static/change-check/documentation gates; retain unresolved checks explicitly.

## Test invocation

Use `pnpm --filter @octoboard/ui exec vitest run <explicit paths>` for focused UI tests. Passing a path after
`pnpm ... test --` did not constrain the audit's run and launched the wider suite. Record the actual command,
selected files and outcome; do not infer a whole-suite pass from a focused check.

## Notes for the developer

Reuse the existing focus handoff, shared Button and collection primitives where applicable. Follow `docs/ui-accessibility.md`, `packages/ui/README.md` “The UI state gallery” and the project’s gallery/change-check workflows. Use gallery fixtures only. This is independent of the visual Alert, copy, field and motion milestones; coordinate shared collection edits with milestone 10.
