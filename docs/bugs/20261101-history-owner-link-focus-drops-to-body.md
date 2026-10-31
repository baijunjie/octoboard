> Severity: Moderate

# Focus drops to the page when an archived session's project-session owner is deleted

## Symptom

With focus on the owner link in an archived session's history, deleting that owner when it is a project session
removes the link and leaves focus on `<body>`, instead of handing it to the history's messages region.

## Reproduction steps

1. Start from the test "hands focus on an archived session's owner link to its history when the owner's record is
   deleted" in `packages/ui/src/App.focusHandoff.test.tsx`, which pins the hand-off for an archived session whose
   owner is a console session above the selection.
2. Change its fixture so the archived session's owner is a project session (no console session sits above the
   selection).
3. Focus the owner link in the archived session's history, then deliver `session_deleted` for the owner.
4. Read `document.activeElement`.

## Expected vs. actual

- Expected: focus moves to the history's messages region, as it does when the owner is a console session —
  `docs/ui-accessibility.md`, section "Focus moved by a change the user did not make": a region unmounted by a daemon
  event hands focus off.
- Actual: focus is left on `<body>`.

## Environment

`packages/ui` at branch `refactor/ui-structure` (also the code on `main` 9155b3a11, whose `App.tsx` holds the same
logic); jsdom under Vitest. Precondition: an archived session selected whose owner is a project session.

## Scope of impact

Keyboard and VoiceOver users reading an archived history whose owner is a project session, when that owner is deleted
(by this or another window, or an agent). Focus lands on the page, where VoiceOver reads the whole web area.
Workaround: move focus back into the window by hand.

## Leads

- Verified (with a scratch probe, since deleted): the owner link is removed by `OwnerLine`'s own render, but the check
  that hands focus off is `useFocusHandoff` in `packages/ui/src/components/focus/useFocusHandoff.ts`, used by
  `TerminalPane` (`packages/ui/src/terminal/TerminalPane.tsx`), and it runs only when `TerminalPane` renders.
- Verified: for a console-session owner, `TerminalPane` renders then only because `App` re-renders on its
  `consoleSessionAboveSelection` selector (`packages/ui/src/App.tsx`), which changes when that owner goes; for a
  project-session owner that selector stays `undefined`, so nothing renders `TerminalPane`.
- Inferred: the hand-off should depend on the owner link's presence rather than on `App` rendering.

## Acceptance criteria

- [ ] With focus on the owner link, deleting a project-session owner moves focus to the history's messages region.
- [ ] The console-session owner case in `App.focusHandoff.test.tsx` still passes.
- [ ] A test pins the project-session owner case.
