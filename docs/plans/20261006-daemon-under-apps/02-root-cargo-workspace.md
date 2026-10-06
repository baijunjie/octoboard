# 02 Root Cargo workspace

> Goal: the daemon and the desktop shell's Tauri crate build as one Cargo workspace from the repository root.
> Completion criteria: a root Cargo workspace lists both crates as members; there is one lockfile and one build directory,
> both at the root, and the per-crate ones are gone; once the sidecar build step has run, building and testing from the
> root covers both crates; the sidecar build step and the release script still produce a desktop application that
> launches with its daemon sidecar; ignore rules and the per-package command docs match the new layout.

## Technical design

- [ ] Root Cargo workspace manifest with the daemon (`apps/daemon`) and the desktop shell's Tauri crate as members.
- [ ] One workspace lockfile replacing the two per-crate lockfiles, and one workspace build directory replacing the two
  per-crate build directories.

## Implementation plan

- [ ] Add the workspace manifest and merge the two lockfiles into one by letting cargo resolve the workspace; confirm both
  crates still build and the daemon's tests pass.
- [ ] Point the sidecar build step at the daemon binary in the workspace build directory, and the release script at the
  application bundle there, then confirm the release script and a `tauri build` still produce a working application.
- [ ] Update ignore rules for the single root build directory.
- [ ] Update the docs that give per-package build and test commands or name a crate's own build directory, and the
  project map's description of the workspace root.

## Notes for the developer

- **Development notes**:
  - Both manifests pin some dependencies to exact versions to stay within the minimum Rust version. In one lockfile a pin
    in one crate constrains the other, so resolution can fail outright rather than just move versions; treat any build or
    test difference as something to investigate, not to paper over.
  - A virtual workspace manifest does not take its dependency resolver from the members' edition; without setting it,
    cargo falls back to the old resolver and warns.
  - Today the sidecar build step reads the daemon binary from the daemon crate's own build directory, and the release
    script looks for the bundle under the Tauri crate's own build directory; with a workspace both move to the root.
  - The Tauri crate checks for its sidecar binary at compile time, so on a clean checkout a root build or test fails until
    the sidecar build step has run once.
- **Reference docs**: `docs/project-map.md`, the READMEs of the daemon and the desktop shell,
  `docs/memory/verifying-the-desktop-ui.md` (names a bare cargo build in the Tauri crate and its build directory).
