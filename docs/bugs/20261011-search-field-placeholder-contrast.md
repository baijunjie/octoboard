> Severity: Moderate

# A search field's placeholder fails AA contrast in the light appearance

## Symptom

The placeholder text of every search field in the app is 3.26:1 against the field's fill in the light appearance, below
the 4.5:1 that WCAG 2.2 AA asks of text.

## Reproduction steps

1. Launch the packaged application in the light appearance.
2. Open the sidebar's project filter, so that its search field shows its placeholder.
3. Measure the placeholder text: take the darkest painted glyph pixel and compare it with the field's fill.
4. Repeat in the dark appearance, and for the Git mode's change-list filter (a search field with the same markup).

The darkest glyph pixel against the fill measures 3.26:1 in light and 4.79:1 in dark.

## Expected vs. actual

- **Expected**: placeholder text meets 4.5:1 against the field's fill in both appearances. `docs/memory/writing-ui-components.md`
  ("The UI meets WCAG 2.2 AA") holds every control and view in `packages/ui` to AA in both appearances, with text at least 4.5:1.
- **Actual**: 3.26:1 in light; 4.79:1 in dark, which passes.

## Environment

- macOS, the packaged application, measured from the real window's painted pixels, not from a downscaled screenshot.
- The field's fill is rgb(235,235,236) on a panel of rgb(250,250,250) in light, and rgb(39,39,42) on rgb(30,31,34) in dark.
- Nothing else about the field fails: entered text is 14.87:1 in light and 14.52:1 in dark, and the search and clear icons
  are 5.07:1 and 5.68:1.

## Scope of impact

Every search field in the app, since they share one markup and one stock colour: the sidebar's project filter, which
already ships, and the Git mode's change-list filter. Light appearance only. The text is a hint, not the field's
accessible name (the field has an `aria-label`), so the field stays usable; the hint is hard to read for low-vision users.
There is no workaround.

## Leads

- **Verified in the code**: the input carries exactly one class, `search-field__input`. `ProjectFilter.tsx`
  (`packages/ui/src/sidebar/`) and `ChangeList.tsx` (`packages/ui/src/browser/`, on the change-filter branch, not yet on
  `main`) both use `SearchField variant="secondary"` with `SearchField.Group`, `SearchIcon`, `Input` and `ClearButton`.
- **Verified in the code**: no stylesheet in the repository (`packages/ui/src/style.css`, `apps/web/app/assets/css/`)
  sets a placeholder colour or styles `search-field`. The colour is HeroUI's own.
- **Verified in the code**: HeroUI's `--field-placeholder` is `var(--muted)`, while `style.css` already overrides the
  light `--muted` to a darker value (L 0.5, documented there as 4.5:1 on its surfaces) — so the placeholder does not
  simply follow `--muted` on the field's fill; the stock value or an opacity applied on top is the cause.
  Not traced further.
- **Inferred**: because the cause is the shared stock colour and not any one field, the fix belongs in the theme and not in
  each field's markup.

## Acceptance criteria

- [ ] The sidebar project filter's placeholder measures at least 4.5:1 against the field's fill in light and in dark,
  measured from the packaged application's painted pixels.
- [ ] The Git mode's change-list filter's placeholder does the same.
- [ ] The fix is not made per field: a search field added later gets a passing placeholder without its own styling.
- [ ] Other fields that draw HeroUI's placeholder (inputs in the dialogs and settings) are checked and pass as well, or are
  recorded as separate tickets if they do not.
