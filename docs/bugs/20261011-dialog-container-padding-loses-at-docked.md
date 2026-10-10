> Severity: Minor

# A dialog that fills the window keeps its narrow margin above the docked breakpoint

## Symptom

Settings and the file viewer ask for a 16 px margin in a narrow window and a 40 px one from the docked breakpoint
up, but the narrow value wins at every width: both are drawn inside a 16 px margin even in a wide window.

## Reproduction steps

1. Run the gallery (`pnpm --filter @octoboard/ui gallery`) and open any scenario that shows the file viewer — "A
   file open from the tree" (`files-viewer`) — in a window at least 1148 px wide.
2. Read the computed `padding-top` / `padding-bottom` of the viewer's positioning container, the element carrying
   `class="modal__container …"` (the dialog's parent).
3. The same for Settings, from the scenario "Settings" or by opening it from the rail.

## Expected vs. actual

- Expected: 40 px at 1148 px and wider. Both call sites ask for exactly that — `Modal.Container` in
  `packages/ui/src/settings/SettingsDialog.tsx` and `Frame.Container` in `packages/ui/src/dialogs/Dialog.tsx` carry
  `sm:p-4 docked:p-10`, and the comment above each says the 16 px is for the narrow window alone. The user
  confirmed on the spot (2026-10-11) that this is worth filing.
- Actual: 16 px at both breakpoints, measured as `padding-top: 16px; padding-bottom: 16px` on the container at
  1440×900.

## Environment

- macOS (Darwin 25.2.0), Apple Silicon; measured in Playwright's `webkit` (WebKit 27.2) against the gallery page,
  on `main` at `cc193ad`.
- Not checked in the packaged application; the macOS window's 1148×600 minimum means it is always in the docked
  layout there, so that is where it shows.
- It predates the connection banner's stacking work of 2026-10-11, which only corrected the two comments that
  claimed the 40 px was in effect.

## Scope of impact

- Every window at or above the docked breakpoint: the two dialogs that fill the window sit closer to the window's
  edges than intended. Nothing is unreachable or unreadable — it is a spacing difference.
- No workaround; it is not under the user's control.
- Other dialogs are unaffected: they add no `sm:` padding of their own, so HeroUI's `sm:p-10` applies and their
  container measures 40 px (verified on the delete-console confirmation).

## Leads

- Verified: in the emitted stylesheet the `docked` media block comes **before** the `sm` block —
  `.docked\:p-10 { padding: calc(var(--spacing) * 10) }` at offset 49541 inside `@media (width >= 1148px)`, then
  `.sm\:p-4 { padding: calc(var(--spacing) * 4) }` at offset 50668 inside `@media (width >= 40rem)`. At 1148 px and
  wider both match, equal specificity, so the later `sm:p-4` wins.
- Verified: the `docked:` utilities on the dialog itself (`docked:h-…`, `docked:w-…`) do apply, since they compete
  with unconditional utilities rather than with an `sm:` twin — the viewer measures 1360×820 at 1440×900, which is
  the docked height.
- Inferred: any `sm:<utility>` paired with a `docked:<same utility>` anywhere in `packages/ui` loses the same way.
  Not surveyed. `docs/memory/writing-ui-components.md` records the sibling trap for `rtl:` against `docked:`; this
  is the same class of problem with `sm:`.

## Acceptance criteria

- [ ] The container of Settings and of the file viewer measures 40 px of padding at 1148 px and wider, and 16 px
      below it.
- [ ] The dialogs' own sizes at both breakpoints are unchanged, and a dialog that adds no padding of its own still
      measures 40 px.
- [ ] The connection banner's strip still clears a dialog's bottom edge at both breakpoints (see "What an open
      dialog, menu or popover covers" in `docs/product/window-layout.md`), whose clearance is the container's
      padding.
- [ ] Any other `sm:` / `docked:` pair in `packages/ui` that loses the same way is either corrected with it or
      recorded as deliberate.
