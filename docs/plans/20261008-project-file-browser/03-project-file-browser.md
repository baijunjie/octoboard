# Project file browsing and navigation

> Goal: ship usable project file browsing from the sidebar through the daemon to the read-only modal.
> Completion criteria: a project with no sessions opens a live file tree; code and supported images open from that
> tree in both browser and packaged desktop clients. Buttons and arrow keys navigate its visible file rows in the
> same order. Project/console transitions, refresh, deletion and reconnect never display another source's content.
> Report behavior, terminal focus and narrow layouts remain usable. Fixture-only viewing does not complete this
> milestone.

Depends on [source contracts](01-source-contracts.md) and [renderer validation](02-renderer-validation.md).

## Technical design

- [ ] The right pane has an explicit owner context: a project browser, a console-session report or no active owner.
  The overview's context transitions do not assign browser ownership to a session or require a session to exist.
- [ ] A project entry point opens its browser without starting or resuming an agent. Selecting a project session
  selects the same project context; changing between sessions of one project does not reset its browser state.
- [ ] A console-session selection routes to its report. Switching consoles clears a browser belonging to the old
  console; deleting the active project clears its browser. No selection silently falls back to a different project.
- [ ] Per-project client state holds the selected file, expanded directories and browser mode. Files is the initial
  mode; later Git milestones extend this state without changing its owner. Persistent preferences do not include
  file bodies.
- [ ] The tree lists the live project scope through the daemon with bounded, on-demand directory loading. Sorting
  and ignored-file rules are defined once by the tree, with explicit loading, empty, unreadable and limited results.
- [ ] Opening a file uses its source-aware identity and shows loading, content or a specific error in the tested
  modal. A failed/deleted file never keeps another file's body under its own name.
- [ ] Previous/next consumes the ordered visible file rows supplied by the tree, under the overview's boundary and
  collapsed-directory rules. Buttons and left/right keys use the same transition; directories are not viewer items.
- [ ] Navigation keeps the modal mounted. Keys remain local to it, respecting text selection, focused controls,
  composition and modifiers; opening/closing does not leak navigation keys to the terminal.
- [ ] Refresh preserves selection by identity, not by list position. A removed or unavailable selection is shown
  explicitly. Changing the active owner closes the old viewer; obsolete reads cannot populate a new context.
- [ ] Reconnect invalidates source-dependent content and reloads the active view. Refresh work is tied to visible
  client context and remains within the source contract's bounds; manual refresh is available.

## Implementation plan

- [ ] Connect project entry, context transitions and per-project state to the existing right-pane layout, including
  visibility toggles, width allocation, drawers, hover panes and F6 region access.
- [ ] Connect bounded directory listings and file reads to the renderer adapter. Verify the complete flow with real
  non-Git and Git projects rather than only supplied viewer content.
- [ ] Connect the tree's file order to modal navigation and reconcile it when a refresh adds, removes or reorders
  files. Verify first/last boundaries and mixed code/image navigation without independent viewer sorting.
- [ ] Verify an empty project, a project without sessions, two sessions in one project, two projects, a console-session
  report, console switching and project deletion. Opening Files must not create or resume a terminal session.
- [ ] Verify rapid navigation with delayed replies, a file deleted or overwritten while open, unreadable content,
  oversized files, partial listings and reconnect. Each result must retain its correct source and limit status.
- [ ] Verify browser and packaged WebView behavior at narrow and docked widths, keyboard-only access, focus
  restoration, light/dark appearance and RTL layout. Run the affected checks and update the shipped behavior docs.

## Notes for the developer

**Reusable capabilities**

- The UI sidebar, pane-layout and region-cycle modules supply project entry surfaces, right-pane geometry,
  visibility, focus hand-offs and F6 navigation.
- The UI daemon-client and store modules supply correlated requests and reconnect invalidation; the dialogs module
  supplies modal reset-key behavior.

**Development notes**

- The current report pane is gated by console-session selection in several layout consumers. Adapt those consumers
  to the active right-pane owner instead of leaving report-specific visibility checks around a project browser.
- Report history and forms belong to the report module. Keep its ownership migration separate from this feature;
  routing a report here must preserve the then-current owner and submission behavior.
- The existing directory picker has a different purpose and only returns directories. Reusing its unrestricted
  directory-selection contract would bypass the project-scoped file-read design.
- Keep the current pane focus protections and localization conventions. A new content mode must remain reachable
  from the terminal through the region cycle, and closing overlays must use the established focus hand-off.

**Reference docs**

- `packages/ui/README.md`, `apps/daemon/README.md`, `apps/daemon/PROTOCOL.md`
- `docs/product/sidebar.md`, `docs/product/window-layout.md`, `docs/product/report-panel.md`
