# 10 Apply shared collection motion and motion preferences

> Goal: apply shared collection motion and motion preferences.
> Completion criteria: Mutable tabs/account collections follow shared motion semantics, and application animations/transitions stop immediately when reduced motion is requested. All listed validation passes, and the independently reviewed result is accepted.

Status: **accepted; implementation not started**. Implementation begins only after the audit passes and the preceding required work is closed.

## Scope

I-03 and I-04 in [the interaction audit](audit-interactions.md), G02 in [the gallery audit](audit-gallery.md).

## Implementation contract

- [ ] Cover shared agent tabs, Console Advanced dynamic agent tabs, shell terminal tabs and quota account lists with stable identities and shared list motion.
- [ ] Preserve drag/reorder focus and reading-order actions, prevent double animation, and retain scroll/recycle/resize/initial-render exclusions.
- [ ] Guard the complete I-04 authored transition inventory, including archive actions/sidebar/resize cues, and cancel already-running height/drag/move effects on preference change.
- [ ] Keep exiting copies inert/aria-hidden, remove them immediately under reduced motion, and retain removal focus handoff.

## Validation requirements

- [ ] Use live fixture add/remove/reorder and availability/account changes in visible collections, both axes/RTL and wrapping, then change motion preference mid-animation. Measure the standard 300ms curves/25% entry/exit against shared behavior. Prove initial load, scroll, recycling and resize do not animate; test focused removal and pending menu interaction. Under reduce, computed transition must be none for nonessential custom effects. Run motion/unit tests, affected/full shared gallery and static checks.
- [ ] Complete the project gallery/static/change-check/documentation gates; preserve failed or unresolved checks explicitly. Do not skip scenarios or weaken assertions.

## Dependencies and handoff

Coordinate shared tab ownership with milestone 08. Dynamic collection behavior and its reduced-motion path share one validation contract; parent may split I-04 cleanup separately if preferred.

## Test invocation

Use `pnpm --filter @octoboard/ui exec vitest run <explicit paths>` for focused UI tests. Passing a path after
`pnpm ... test --` did not constrain the audit's run and launched the wider suite. Record the actual command,
selected files and outcome; do not infer a whole-suite pass from a focused check.

## Notes for the developer

Reuse the existing shared components and mechanisms named in the linked finding record. Follow `docs/ui-accessibility.md`, `packages/ui/README.md` “The UI state gallery”, `docs/voiceover-cases.md`, and project gallery/change-check workflows. Only gallery fixtures may stage data. Read the exact baseline evidence and recheck caller inventory after rebasing; other owners’ pending changes are not certified by this audit.
