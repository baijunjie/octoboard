> Severity: Moderate

# The connection banner's Retry cannot be reached from the keyboard while a dialog is open

## Symptom

With a dialog open and the control connection down, no key reaches the banner's **Retry** — F6 never leaves the
dialog and Tab only cycles inside it — so a keyboard user has no way back without first dismissing the dialog.

## Reproduction steps

1. Run the gallery (`pnpm --filter @octoboard/ui gallery`) and open the scenario "Reconnect budget spent with a
   dialog open" (`daemon-lost-behind-dialog` in `packages/ui/src/gallery/fixtures/connection.ts`): the file viewer is
   open on `src/main.ts` and the banner offers Try again along the bottom edge. The same state is reached in the
   application by dropping the control connection for longer than about two seconds while a dialog is open.
2. Press F6 repeatedly, and read `document.activeElement` after each press. Shift+F6 too.
3. Press Tab repeatedly and read it again.

## Expected vs. actual

- Expected: Retry is reachable from the keyboard while a dialog is open. `docs/product/application-lifecycle.md`'s
  "Losing the daemon connection" presents Retry as the way back after the automatic attempts and names no state in
  which it is unavailable, and `docs/product/moving-focus-between-regions.md` makes F6 the way between the window's
  regions. This rests also on a position the user confirmed on the spot (2026-10-11), when the mouse half of the same
  defect was fixed and this half was split off.
- Actual: focus starts on the viewer's own `<section>`, and stays there through six F6 presses and three Shift+F6
  presses. A fourteen-press Tab walk cycles the viewer's own five stops — Close → Wrap lines → the focusable code
  region ("Contents of main.ts") → Previous file → Next file → back to Close — and never reaches the banner.

## Environment

- macOS (Darwin 25.2.0), Apple Silicon; measured in Playwright's `webkit` (WebKit 27.2, viewport 1440×900) against
  the gallery page, with the banner's mouse-reachability fix in place.
- Not checked in the packaged application, in a browser, or on Linux; no particular project or repository state is
  needed.

## Scope of impact

- Any keyboard-only user whose control connection drops while a dialog is open. The workaround is to dismiss the
  dialog first (Escape), after which F6 reaches the banner as usual.
- The same F6 suppression applies to HeroUI's toast region, so a toast's own controls are equally unreachable from
  the keyboard while a dialog is open; whether that is wanted is Unknown.

## Leads

- Verified: `packages/ui/src/layout/useRegionCycle.ts:80` and `:101` swallow F6 outright while `MODAL_OPEN` matches.
  The hook's own doc comment gives the reason — "nor react-aria's own landmark navigation, which would move focus out
  from under an open dialog or menu" — so the suppression is deliberate and relaxing it for one region is a decision,
  not an oversight.
- Verified: the button itself is ready for the keyboard — it is marked a React Aria top layer, so it is outside the
  `inert` subtree an open modal creates, and measured in that state `retry.closest("[inert]")` is `null`,
  `retry.tabIndex` is 0 and `retry.focus()` lands. The F6 guard above is the only thing left between a keyboard and
  the button, together with React Aria's `FocusScope` holding Tab inside the dialog.
- Verified: a focus excursion out of a react-aria modal and back costs the dialog its Escape. With the F6 guard
  lifted as a probe, F6 landed on Try again in one press and Enter activated it with the dialog still open, but after
  Tab returned focus into the dialog, Escape no longer closed it and no `keydown` reached either the dialog or its
  backdrop, although `document.activeElement` reported a button inside the dialog. A press on Retry with the mouse
  does not do this, since `preventFocusOnPress` keeps focus where it was.
- Verified, with the guard lifted as a probe as above: HeroUI's toast region survives the same excursion intact —
  F6 to it over an open dialog, then Escape, returned focus to the dialog and left both open. It is a proper
  react-aria overlay with its own focus scope, registered in react-aria's overlay stack, which the banner is not.
- Verified: `useFocusHandoff`'s targets are both unreachable while a dialog is open — the terminal's element and the
  top bar's first button are inside an `inert` subtree, and `.focus()` on either is a no-op — so the banner going
  away from under focus in that state would drop focus to `<body>`, which by this project's own rule
  (`docs/memory/writing-ui-components.md`) costs the dialog its Escape and its Tab containment.
- Inferred: giving the banner its own react-aria overlay (an `Overlay` / `FocusScope`, as HeroUI's toast region has)
  is what would let focus enter and leave it without damaging the dialog. Not built.

## Acceptance criteria

- [ ] With a dialog open and the connection `closed`, F6 reaches the banner and Retry can be activated from the
      keyboard, with the dialog still open afterwards.
- [ ] After focus has been to the banner and back, the dialog still closes on Escape and still contains Tab.
- [ ] When the banner leaves while a dialog is open, focus lands somewhere sensible rather than on `<body>`.
- [ ] F6 still does not move focus out from under an open dialog or menu in any other case, which is what the
      suppression in `useRegionCycle.ts` is there for.
- [ ] `docs/product/moving-focus-between-regions.md` and `docs/product/application-lifecycle.md` still describe what
      the application does, amended if the fix changes it.
