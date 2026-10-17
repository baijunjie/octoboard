# Split right pane

> Goal: when a project owns the right pane, the pane is a vertical stack of panels the user can show, hide, resize
> and maximize one by one.
> Completion criteria: the stack's state handles several kinds stacked, resized, maximized and closed (unit tests);
> the Files panel behaves as today's project browser inside the stack, in docked, floating and drawer forms; it is a
> rounded card with the panel card edge that fills the pane, with no frame around it; it has a header with title,
> actions, maximize and close; the top bar shows the pane's own toggle as today and, while a project
> owns the pane, one toggle per panel kind; which panels are open and their heights survive a reload; the Files panel
> is its own F6 stop. A report-owned pane is unchanged. Verified by unit tests, gallery scenarios of the Files panel in
> each form, and keyboard checks of F6. Stacking two real panels is verified in milestone 5, once a second kind
> exists.

## Technical design

- [ ] A panel kind registry: each kind gives its title, icon, header actions and content, so a later kind is one entry.
- [ ] Each panel is its own rounded card with the panel card edge from milestone 2. The cards are the right pane: in
      docked, floating and drawer forms the pane has no frame or edge of its own around them. They fill the pane edge to
      edge with no margin, separated only by a gap that holds the resize handle. The gap shows the sidebar's background
      in every form.
- [ ] Stack state: the ordered open panels and their share of the height, with each panel's minimum height; one panel
      may be maximized at a time.
- [ ] A vertical resize handle in the gap between neighbouring panels, with the same keyboard steps, Home / End,
      double-click reset and pointer capture as the existing horizontal handle.
- [ ] A panel header with title, kind-specific actions, maximize / restore and close, each icon-only control with a
      tooltip and an accessible name, toggles carrying their expanded state.
- [ ] Top-bar toggles, one per panel kind, beside the pane's own toggle, present only while a project owns the right
      pane, each labelled with what pressing it does. The pane's own toggle keeps showing and hiding the whole pane,
      the hover float-in and the drawer.
- [ ] Persisted preferences: open panels and heights, per client, beside the right pane's width.
- [ ] Each panel is a named region and an F6 stop with its own landing spot.

## Implementation plan

- [ ] Generalize the resize handle to either axis, keeping the horizontal one's behaviour.
- [ ] Build the stack container and panel header inside the right pane frame, keeping its docked, floating and drawer
      behaviour and its sidebar-coloured surface behind the cards, while dropping its own edge in the project view,
      where the cards carry it.
- [ ] Move the project browser in as the Files panel; its own header becomes the panel's header.
- [ ] Add the per-kind toggles and the persisted preferences.
- [ ] Extend the F6 region cycle to the stack.
- [ ] Add gallery scenarios for the Files panel docked, floating and as a drawer.

## Notes for the developer

- **Reusable capabilities**: the right pane frame (docked / floating / drawer), the pane resize handle and width
  clamping, panel visibility preferences and pane toggles, the top bar's chrome buttons, the shared row and icon
  controls, the F6 region cycle.
- **Development notes**:
  - Headers on the window's top line keep `--pane-header-height` so their bottom lines run on; the top card's header
    has to allow for its own edge to keep that line. Headers further down the stack are a new tier and need their
    height settled.
  - The pane's 16px inset from milestone 1 is measured from the card's edge.
  - Only icon-only controls go on the window material, and `docs/product/window-layout.md` says nothing else is in the
    bar today; it needs updating with the toggles.
  - Docked panes resizing the agent terminal is existing behaviour; resizing panels inside the stack must not resize
    it.
  - Closing a panel that holds focus must not drop focus to the page body.
  - Preference keys go through the shared key list; the gallery's scenario preferences need the new ones.
  - Focus rings come from the shared focus-visible helper, not CSS `:focus-visible`.
- **Reference docs**: `docs/product/window-layout.md`, `docs/product/project-pane.md`,
  `docs/product/moving-focus-between-regions.md`, `docs/ui-accessibility.md`,
  `docs/memory/focus-and-key-handling-in-the-ui.md`, `docs/memory/what-goes-on-the-window-material.md`,
  `docs/memory/keeping-the-ui-responsive-in-webkit.md`.
