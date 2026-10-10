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
