# 10 Apply shared collection motion and motion preferences

> Goal: apply shared collection motion and motion preferences.
> Completion criteria: Mutable tabs/account collections follow shared motion semantics, and application animations/transitions stop immediately when reduced motion is requested. All listed validation passes, and the independently reviewed result is accepted.

Status: **ready; not started**, except the quota account rows, which are blocked (see "Dependencies and handoff").

## Scope

The accepted findings and current layout dependency are reproduced below so this brief remains self-contained.

- **Mutable collections (I-03):** shared agent tabs (`AgentTabList.tsx:72-76`), Console Advanced agent tabs
  (`ConsoleLaunchFields.tsx:87-99`), Shell tabs (`ShellTerminalPanel.tsx:253-268`) and quota-account lists
  (`quota/RailQuotaEntry.tsx:403-412`) change while mounted without shared collection motion. Settings account rows
  already have motion (`f9daa976b`, `AgentAccountsSection.tsx:303`) and are out of scope. Keep stable identities, lazy mounting, React Aria
  tab focus/selection, drag/reorder, menu focus return (`ActionMenu`'s `onClosed` and `browser/focusRow.ts`'s `focusRowControl` exist if the tab menus need keyboard focus return), shell close handoff and quota height behavior. Cover
  add/remove/reorder and live availability/account changes on both axes and RTL. Exclude initial render, scroll,
  recycling and resize; removed controls disappear immediately and exit copies remain inert.
- **Reduced motion (I-04/G02):** custom transitions remain at `sidebar/rows.tsx:91`, `sidebar/Sidebar.tsx:596`,
  `archive/ArchiveView.tsx:296` (rewritten by `e46badae9`), `components/ResizeSeparator.tsx:169`,
  `dialogs/useHeightMotion.ts:39` and `components/tabDrag.ts:35,52`. Height motion checks preference only before
  starting; drag snapshots preference at drag start. Guard these effects and cancel active height/drag motion when the
  preference changes. Do not duplicate `useListMotion`. New since the audit: the view-switch Web Animation at
  `Sidebar.tsx:281/294` reads `matchMedia` directly and is not cancelled when the preference changes; use
  `prefersReducedMotion()`.
- **Current Shell layout:** keep the 08 one-row contract. The visual title stays hidden until the viewport is at
  least 1168px and the named header container has at least 352px of content; the accessible heading and DOM nodes
  remain. The narrow pane floor is 336px only while Terminal remains in the column, otherwise 320px; saved widths
  are not rewritten. Preserve the full focus ring, selected Files/Changes identity and real F6 entry in RTL.

## Implementation contract

- [ ] Cover shared agent tabs, Console Advanced dynamic agent tabs, shell terminal tabs and quota account lists with stable identities and shared list motion.
- [ ] Preserve drag/reorder focus and reading-order actions, prevent double animation, and retain scroll/recycle/resize/initial-render exclusions.
- [ ] Guard the complete I-04 authored transition inventory, including archive actions/sidebar/resize cues, and cancel already-running height/drag/move effects on preference change.
- [ ] Keep exiting copies inert/aria-hidden, remove them immediately under reduced motion, and retain removal focus handoff.

## Validation requirements

- [ ] Add the below-782px floating panel layout to the Shell-tab validation (its own motion is already guarded). Use live fixture add/remove/reorder and availability/account changes in visible collections, both axes/RTL and wrapping, then change motion preference mid-animation. Measure the standard 300ms curves/25% entry/exit against shared behavior. Prove initial load, scroll, recycling and resize do not animate; test focused removal and pending menu interaction. Under reduce, computed transition must be none for nonessential custom effects. Browser checks can emulate `reducedMotion`; native checks need the user to toggle Reduce motion by hand (`docs/memory/driving-and-reading-the-real-app-window.md`). Run motion/unit tests, affected/full shared gallery and static checks.
- [ ] Complete the project gallery/static/change-check/documentation gates; preserve failed or unresolved checks explicitly. Do not skip scenarios or weaken assertions.

## Dependencies and handoff

Preserve content-width, start-aligned tab lists and their complete focus-ring, clipping and scroll clearance.
Keep surviving tab nodes, selection and Files/Changes mode stable during resize and reallocation, including real
F6 entry and RTL behavior. Shell remains on one header row; retain the title allocation in
`docs/product/right-pane-panels.md`, "A panel's header", and the conditional pane minimum in
`docs/product/window-layout.md`, "Below 1168 px: the sidebar becomes a drawer".

- **Blocked: until `feat/agent-named-quota-windows` is merged.** This applies only to the quota account rows
  (`quota/RailQuotaEntry.tsx`, `AccountQuotaSummary.tsx`), because that branch edits `AccountQuotaSummary.tsx`,
  `RailQuotaEntry.test.tsx` and `AgentAccountsSection.test.tsx`. The rest of this milestone may proceed. Once it
  lands, decide whether quota meters appearing and disappearing (`AccountQuotaSummary.tsx:128-136`) are in scope.
- 09 also edits `Sidebar.tsx`; merge the two one at a time.

Dynamic collection behavior and its reduced-motion path share one validation contract; parent may split I-04
cleanup separately if preferred.

## Test invocation

Use `pnpm --filter @octoboard/ui exec vitest run <explicit paths>` for focused UI tests. Passing a path after
`pnpm ... test --` did not constrain the audit's run and launched the wider suite. Record the actual command,
selected files and outcome; do not infer a whole-suite pass from a focused check.

## Notes for the developer

Original finding records are in Git: `git show 7d7fdb2da^:docs/plans/20261020-ui-standard-audit/{audit-states,audit-interactions,audit-gallery}.md`. Reuse the existing shared components and mechanisms named there. Follow `docs/ui-accessibility.md`, `packages/ui/README.md` “The UI state gallery”, `docs/voiceover-cases.md`, and project gallery/change-check workflows. Only gallery fixtures may stage data. Read the exact baseline evidence and recheck caller inventory after rebasing; other owners’ pending changes are not certified by this audit.
