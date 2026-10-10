# Surfaces and light-mode contrast in the viewer and Git mode

> Goal: the viewer's code area and the project pane's Git mode have clear boundaries and enough contrast in light mode
> as well as dark.
> Completion criteria: in the packaged app's light mode, the viewer's code area is visibly bounded from the dialog, and
> the Git mode's controls, sections and rows are distinguishable from the panel; dark mode is unchanged or improved;
> the chosen surfaces come from HeroUI's components rather than hand-set backgrounds.

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
