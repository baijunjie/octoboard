# HeroUI Components in Place of Hand-built Ones

> Goal: every hand-built element that HeroUI 3 covers is replaced by the HeroUI component, and every element that
> stays hand-built says why at the component.
> Completion criteria: no raw `<button>` remains except where a comment states why HeroUI's `Button` cannot be used;
> each replaced element behaves as before (or as noted below) in the running app, light and dark, by mouse and by
> keyboard; `pnpm --filter @octoboard/ui build` passes.

### Technical design

- [ ] The directory picker's entry list becomes HeroUI's `ListBox`: one tab stop with arrow-key navigation instead of
  one tab stop per entry, keeping the parent-directory entry, the empty state, and moving into a directory on
  activation.
- [ ] The directory picker's "git" tag becomes HeroUI's `Chip`.
- [ ] The settings dialog's section navigation becomes HeroUI's vertical `Tabs`: arrow-key navigation and the selected
  state exposed as a tab; focus on open and the return of focus after a section's control unmounts keep working,
  targeting the selected tab.
- [ ] The sidebar tree's vertical scroll area becomes HeroUI's `ScrollShadow`, with the fade size kept at the current
  24 px and the scrollbar look checked against the rest of the window.
- [ ] The working-status spinner becomes HeroUI's `Spinner`, keeping the status's accessible name.
- [ ] Confirmation dialogs (the quit confirmation, the other confirmations, the workspace-trust prompt) use HeroUI's
  `AlertDialog`, set back to closing on Escape and an outside press as they do today.
- [ ] The narrow-mode dimming layer keeps its native `<button>`, with a comment saying why HeroUI's `Button` cannot be
  it; every other element listed as staying hand-built in the overview gets a comment stating why HeroUI's is not used.

## Notes for the developer

- **Reusable capabilities**: the shared dialog frame and its focus-restore helpers (they carry over to `AlertDialog`);
  the edge-fade component, which stays for single-line labels.
- **Development notes**: HeroUI's `Tabs`, `ListBox` items and `Disclosure` triggers take focus on press; that is fine
  inside a modal dialog, but nothing outside one may pull keyboard focus off the terminal. The authoritative list of
  what HeroUI 3 provides is the installed package's component directory.
- **Reference docs**: `docs/memory/writing-ui-components.md`, `packages/ui/README.md`, `docs/product/settings.md`.
