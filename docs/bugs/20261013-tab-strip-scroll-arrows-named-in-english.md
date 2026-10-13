> Severity: Minor

# A tab strip's scroll arrows are named in English in every language

## Symptom

The two arrow buttons the component library draws at the ends of a tab strip that has more tabs than fit carry the
accessible names "Scroll tabs up" / "Scroll tabs down" (a vertical strip) and "Scroll tabs left" / "Scroll tabs right"
(a horizontal one), hardcoded in English in the library rather than taken from the application's message catalogs, so
a screen reader announces them in English whatever language the window is in.

## Reproduction steps

1. Set the application's language to Simplified Chinese (Settings → General → Language, or the system language with
   the setting left on its default).
2. Narrow the window below 1148 px, so the layout is the narrow one and the Settings dialog's list of sections is a
   row of tabs along the top.
3. Open Settings. Narrow the window further until that row has more tabs than fit, so the library shows its scroll
   arrows at the row's ends (they are `display: none` until then).
4. Read the arrows' accessible names — with VoiceOver, or from the accessibility tree, or from the rendered
   `aria-label` of the two `button` elements inside `[data-slot=tabs-list-container]`.
5. The Git mode's Uncommitted / Compare row is the second place with such a strip, at any window width narrow enough
   for its two tabs to overflow.

## Expected vs. actual

- Expected: both names are in the window's current language. Every string the user reads goes through the message
  catalogs — "Every string the user reads goes through the catalog, in a shape a translation can follow" in
  `docs/memory/writing-ui-components.md`, and "What follows the language" in `docs/product/language.md`, which exempts
  only text aimed at the agents and a report page the model wrote.
- Actual: both names are English in all 17 languages.

## Environment

`@heroui/react` 3.2.6, as installed. The desktop application and the browser alike; both appearances; no data
preconditions beyond a window narrow enough for a tab strip to overflow.

## Scope of impact

A screen reader user in a language other than English, on the two tab strips that can overflow: the Settings dialog's
row of sections in the narrow layout, and the Git mode's Uncommitted / Compare row. The arrows are `tabIndex={-1}`, so
they are not Tab stops, and the tabs themselves are reachable and correctly named, so the strip can still be used —
the arrows are an extra, mislabelled route rather than the only one. No workaround for the user.

## Leads

- Verified: the names are JSX literals in `packages/ui/node_modules/@heroui/react/dist/components/tabs/tabs.js`, on
  the two `button` elements the tab list container renders after its scroller.
- Verified: nothing reaches them from outside. `TabListContainerProps` is `{children?, className?, render?}` plus plain
  div attributes, and the container's remaining props are spread onto the container element, never onto the buttons;
  `TabsContext` carries only the orientation and the theme slots and is not exported.
- Verified: the strings bypass react-aria's localization entirely — the library's tabs module imports no string
  formatter, and the package ships no translation table — so the locale the application already provides through
  react-aria's `I18nProvider` has no effect on them.
- Inferred: the library's `render` prop on the tab list container receives the rendered children, among them those two
  buttons, which a caller could rebuild with names of its own. That route depends on the order and the element types of
  children the library does not document, so it would need a test holding it in place against a library upgrade.
- Inferred: a newer `@heroui/react` than 3.2.6 may have fixed this upstream; not checked, no network call was made.
- Verified: the two call sites are `packages/ui/src/settings/SettingsDialog.tsx` and `packages/ui/src/browser/GitView.tsx`,
  the only two that give `Tabs` a list container. A third tab strip, the project browser's Files / Git row, uses a bare
  tab list with no container, so it has no arrows and no fade.

## Acceptance criteria

- [ ] Both arrows of a vertical tab strip, and both of a horizontal one, carry accessible names from the message
      catalogs, in the window's current language.
- [ ] The names are right in both orientations, since the Settings dialog's strip is vertical in the docked layout and
      horizontal in the narrow one.
- [ ] Both catalogs carry the new messages, and the narrow-window Settings strip and the Git mode's row both announce
      them.
- [ ] Something fails if a library upgrade breaks whatever route the names take, rather than the names silently
      reverting to English.
- [ ] `docs/product/scrolling-surfaces.md` says of a tab strip that "the size and the chevrons are the component
      library's own, as drawn, and are not the application's"; if the names stop being the library's, that sentence is
      brought in line.
