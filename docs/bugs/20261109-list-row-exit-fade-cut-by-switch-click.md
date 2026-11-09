> Severity: Minor

# A row removed by clicking a switch in the same list vanishes without its exit fade

## Symptom

When a mouse click on a switch inside a list that uses the shared list motion removes a row of that list, the removed
row disappears almost at once instead of fading out over 300 ms.

## Reproduction steps

1. In `packages/ui`, run `pnpm gallery --port <free port>` and open
   `gallery.html?scenario=settings-app-behavior-desktop&theme=light&lang=en`.
2. In Settings > App behavior, Prevent sleep is on and the "When to prevent sleep" row is shown below it.
3. Click the Prevent sleep switch with the mouse.
4. Watch the "When to prevent sleep" row: it is gone after a few milliseconds (about 2 ms in WebKit, about 96 ms in
   Chromium) instead of fading out to the right over 300 ms.
5. For comparison, focus the switch first and toggle it with Space: the row then fades out over the full 300 ms.

## Expected vs. actual

- Expected: the removed row fades out to the right as an inert copy over 300 ms, whatever input removed it
  (`docs/ui-standard.md`, section "Dynamic collections").
- Actual: with a mouse click the exit copy is dropped almost immediately; with the keyboard it plays in full.

## Environment

Gallery over the fixture daemon, Playwright WebKit and Chromium headless; any list using `useListMotion` whose rows
contain a switch that, when clicked, removes a row of the same list. The desktop app (WKWebView) is expected to behave
like WebKit.

## Scope of impact

Every user who turns off a switch with the mouse where turning it off removes a dependent row (Prevent sleep's "When to
prevent sleep" row is one). Purely visual; no function is lost. No workaround other than using the keyboard.

## Leads

- Verified: clicking the switch focuses its visually hidden input; the 1 px `overflow: hidden` wrapper around that input
  scrolls by about 11 px, which fires a `scroll` event inside the list.
- Verified: `packages/ui/src/components/motion/useListMotion.ts` listens for `scroll` on the list in the capture phase
  (`onScroll`), and for a nonvirtualized list it clears every exit copy on any such event.
- Inferred: the handler cannot tell that scroll from a real scroll of the list or of a scroller between it and its
  rows, so any list with a switch inside is affected the same way.

## Acceptance criteria

- [ ] Clicking the Prevent sleep switch in `settings-app-behavior-desktop` lets the "When to prevent sleep" row fade
      out over the full 300 ms in WebKit and Chromium.
- [ ] Scrolling the list itself (or a scroller between the list and its rows) during an exit still clears the exit
      copies, as before.
- [ ] Toggling with the keyboard still plays the full fade.
