# Shadow edges and groove dividers

> Goal: the UI has no decorative outline borders: surfaces are separated by a thin shadow, panel cards by a hairline
> and that shadow, and dividers read as a groove pressed into the surface.
> Completion criteria: every other decorative outline border is drawn by the 4px thin shadow instead; the floating pane
> and the drawer are edged by a 0.5px hairline and the shadow (a 1px hairline on a 1x display); every divider line is a
> 0.5px double line, a dark line beside a light one, on a high-density display and a single 1px dark line on a 1x
> display; both have light and dark theme values; borders that identify a control and borders that show a selected state
> keep 3:1 contrast. Verified in the UI state gallery across the window layout, sidebar, project pane and report groups,
> in both themes, on a 2x and a 1x display, and with `pnpm bench:wkwebview` showing no regression.

## Technical design

- [ ] The shadow edge is the existing 4px panel shadow token, with its light and dark values, applied wherever a
      decorative outline border stands today.
- [ ] A groove divider token: two adjacent 0.5px lines, the first darker and the second lighter than the surface, so
      the divider reads as recessed; light and dark theme values; horizontal and vertical forms; on a 1x display a
      single 1px dark line.
- [ ] A panel card edge: a 0.5px hairline outline together with the shadow edge, light and dark theme values, a 1px
      hairline on a 1x display. It is decorative, not a control's boundary, so it is not held to 3:1.
- [ ] The floating pane and the drawer take the panel card edge in place of their 1px outline. Milestone 3 removes it in
      the project view, where the panel cards carry it.
- [ ] Other decorative outline borders, such as hand-built cards', replaced by the shadow edge.
- [ ] Divider lines (header bottom lines, section separators, list separators) replaced by the groove divider.

## Implementation plan

- [ ] Inventory the borders and dividers in the UI and sort each into shadow edge, panel card edge, groove divider, or
      a control or selected-state border that stays.
- [ ] Add the groove and panel card edge tokens and a shared way to apply each, including the 1x fallbacks.
- [ ] Move the layout first, then the sidebar, the project pane and the report pane, checking each in the gallery.
- [ ] Update the accessibility standard's card outline rule to the shadow edge.
- [ ] Sweep the gallery scenarios this touches and run the WebKit benchmark.

## Notes for the developer

- **Reusable capabilities**: the panel shadow token, which already sets the content panel and the docked panes apart
  (docked panes have no dividing line between them today, only contrast and that shadow); the app's separator colour
  override; the theme tokens with their dark-mode values; the gallery's layout groups; the WebKit benchmark.
- **Development notes**:
  - HeroUI's default styles take precedence; a HeroUI component's look, borders included, does not change.
  - `docs/ui-accessibility.md` prescribes a separator outline for hand-built cards, with an accent border at 3:1 for
    their selected state. The outline rule changes with this milestone (the main agent maintains that doc); the
    selected-state border stays at 3:1.
  - The floating card's header height is derived from the docked one minus the card's 1px border; the hairline changes
    that value and how the floating and docked header lines line up. Either draw the hairline so it takes no layout
    space, or compensate for it, since it is 0.5px or 1px depending on display density.
  - The panes' header bottom lines run on as one line across the panes; the groove divider must keep that.
  - Shadows and extra lines add restyle and paint cost in WKWebView; measure rather than assume.
  - The split right pane and terminal panel that follow build their edges and dividers on these tokens.
- **Reference docs**: `docs/ui-accessibility.md` ("Hand-built rows and cards"), `docs/product/window-layout.md`,
  `docs/memory/styling-the-ui.md`, `docs/memory/meeting-the-accessibility-bar.md`,
  `docs/memory/keeping-the-ui-responsive-in-webkit.md`, `docs/memory/what-goes-on-the-window-material.md`.
