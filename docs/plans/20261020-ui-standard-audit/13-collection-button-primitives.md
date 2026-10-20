# 13 Use the shared button primitive for collection chevrons

> Goal: use the shared button primitive for collection chevrons.
> Completion criteria: Files/Git tree chevrons follow the shared HeroUI Button contract, preserving React Aria collection slot, expansion semantics, row focus and equivalent row action. All listed validation passes and the result is independently reviewed.

Status: **accepted for parallel implementation** in its own worktree. The user authorized 04, 12 and 13 to
proceed independently; closure of 04–11 is not a start prerequisite. Independently verify this scope and wait for
the coordinating session’s local-integration authorization, after 04’s first integration slot.

## Scope

I-08 in [the interaction audit](audit-interactions.md); source and actual traversal evidence in evidence/interaction-probes.json.

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
