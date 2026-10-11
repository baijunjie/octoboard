# Surfaces and light-mode contrast in the viewer and Git mode

> Goal: the viewer's code area and the project pane's Git mode have clear boundaries and enough contrast in light mode
> as well as dark.
> Completion criteria: in the packaged app's light mode, the viewer's code area is visibly bounded from the dialog, and
> the Git mode's controls, sections and rows are distinguishable from the panel; dark mode is unchanged or improved;
> the chosen surfaces come from HeroUI's components rather than hand-set backgrounds.

## Handoff

From milestone 05 (filtering the change list by file name):

- [ ] The change list's filter field barely separates from the Git panel: its fill measured about 1.2:1 against the
  panel in both themes (light `#E7E7EA` on `#FAFAFA`, dark `#1E1F22` on `#0F0F12`) in the packaged app, so the field
  reads by its icon and placeholder rather than by its own shape. That is HeroUI's own `secondary` `SearchField`
  styling, and the sidebar's project filter has it too, so decide it here along with the panel's other surfaces
  rather than per field. Everything else about the field passes AA: entered text 14.87:1 light / 14.52:1 dark, its
  icons 5.07:1 / 5.68:1, the no-match line 5.78:1 / 6.28:1. The placeholder's own light-mode contrast is a separate,
  app-wide defect with its own ticket, `docs/bugs/20261011-search-field-placeholder-contrast.md`, and is not this
  milestone's to fix.

From milestone 07 (a document view for Markdown in the viewer):

- [ ] The rendered document's frame sits on the code frame's background, so it has the same missing boundary against
  the dialog in light mode. Move it onto whichever HeroUI surface the code area moves to, rather than leaving the two
  frames on different footings.

From milestone 08 (the viewer header's layout and tags):

- [ ] The neutral chip fill was fixed for the viewer's dialog only, by overriding HeroUI's whole `--default` family
  on it (`.dialog-fills` in `packages/ui/src/style.css`, applied to the `size="viewer"` dialog). Every dialog sits on
  the same `--overlay`, so a neutral soft chip elsewhere still has the fill that was diagnosed as not reading as a
  tag — `DirectoryPicker.tsx:145` draws one. Decide here whether the override belongs to dialogs in general, along
  with the panel's other surfaces, rather than per dialog. The viewer's measured values are in the `.dialog-fills`
  comment and are the reference.
- [ ] The viewer's view-control group marks its selected button with a fill difference of 1.03:1 in light and 1.35:1
  in dark, leaning on the label's colour change and `aria-pressed` to carry the state. Not a regression from that
  milestone — it was that way before and improved slightly in dark — but "not by colour alone" and 1.4.11's
  requirement for a state are both thin there, and the fills are this milestone's subject.

## Technical design

- [ ] The viewer's code, diff and document areas sit on a HeroUI Surface whose variant gives a visible boundary in
  both themes.
- [ ] The Git mode panel's containers (branch selector row, sections, list) use HeroUI surfaces and tokens so their
  contrast holds in light mode.

## Implementation plan

- [ ] Replace the hand-set code-area container with the Surface, then check every viewer state (file, diff, sections,
  image, unsupported) in both themes.
- [ ] Rework the Git mode panel's containers the same way, and compare with the Files mode so the two modes match.

## Notes for the developer

**Reusable capabilities**

- HeroUI's Surface component (already used by the terminal pane); the app's panel, separator and selection tokens.

**Development notes**

- Today neither the viewer's code area nor the Git and Files panels use a Surface; they are plain containers on the
  panel and overlay tokens, which is why the light theme loses the boundaries the dark theme happens to show.
- The border and contrast rules in the UI-components memory apply.

**Reference docs**

- `docs/memory/writing-ui-components.md`, `docs/product/project-pane.md`, `docs/product/project-pane-git-mode.md`
