# 04 Cutover

> Goal: the macOS application runs the Vue UI and the React code is gone, so that polishing continues on the Vue UI.
> Completion criteria: the desktop application builds and launches with the Vue UI and does everything the product docs
> describe, checked on a real build; no React code or dependency remains.

## Technical design

- [ ] The desktop shell loads the Vue UI package.
- [ ] The React UI and its dependencies are removed.

## Implementation plan

- [ ] Switch the shell over, run the whole product-doc parity pass on a real build, then delete the React code.
- [ ] Record, as the starting list for polish, whatever the parity pass found that was not a regression.

## Notes for the developer

- **Development notes**: this is the milestone that needs a real build; do not delete React before the parity pass is
  done. Parity is the only gate: do not hold the cutover back for polish, which resumes on the Vue UI afterwards.
- **Reference docs**: `docs/memory/verifying-the-desktop-ui.md`, `docs/product/application-lifecycle.md`.
