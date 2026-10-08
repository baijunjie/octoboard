# Project file browser and Git review

## Problem

The project's right sidebar should default to a code tree for its current branch. Opening a file shows its
contents in a read-only modal viewer. The viewer handles common source-code files and images, and offers previous
and next controls bound to the left and right arrow keys, following the directory tree's top-to-bottom file order.

The same project panel supports Git review: comparing any local branch with any other local branch, and inspecting
uncommitted files in staged and unstaged groups. Linked worktrees can be selected to inspect their own uncommitted
files. The panel belongs to the project, not to an individual session.

## Settled requirements

- File browsing is the default panel content.
- The file viewer is a modal, supports code and images, and provides no editing capability.
- Previous and next navigation follows the tree's file order and is available through buttons and left/right keys.
- Local branch comparisons accept arbitrary local branches as both endpoints.
- Uncommitted changes distinguish staged from unstaged files.
- Each linked worktree's uncommitted files can be inspected separately.
- Project browser context is independent of session ownership.

## Plan outline

The viewer and its navigation can be verified with supplied file content and an ordered file list before panel
routing or the default tree source is settled. Worktree changes can likewise be verified for an explicitly selected
worktree. The remaining integration milestones need the decisions below before their completion criteria can be
written. These pending milestones are deliberately not specified yet.

1. [Read-only code and image viewer](01-read-only-viewer.md).
2. [Previous and next file navigation](02-file-navigation.md).
3. [Uncommitted changes for a selected worktree](03-worktree-changes.md).
4. Pending: project-owned right-pane integration and the default code tree.
5. Pending: local branch comparison and its viewer integration.

## Decisions needed before the remaining milestones

- **Default uncommitted source:** does "main branch" mean the project's main working directory, regardless of the
  branch checked out there, or the worktree that has `main` / `master` checked out? Uncommitted changes belong to a
  worktree, and a branch without a checkout has none to display.
- **Default code-tree source:** live files, including uncommitted changes, or only the files committed at the current
  branch's `HEAD`? This also determines which file contents the modal opens by default.
- **Right-pane routing:** does the right pane offer Files / Git / Report modes, follow project versus console context,
  or replace the existing report pane? Define how a project is opened when it has no session, and what identifies the
  displayed project when a console session is selected.
- **Branch comparison meaning:** compare the two branch tips directly, or compare their merge base with the target
  branch? These can produce different changed-file sets.

## Open

These details can be settled while developing the independent milestones; they do not alter their goals.

- Which code-viewing library to select after checking its compatibility with the packaged WebView.
- Which image formats the first version guarantees, and whether zoom/pan is needed beyond fitting an image in the viewer.
- Whether previous/next includes files in collapsed directories, and the exact tree sorting convention. The navigation
  milestone consumes the ordered list provided by the tree rather than inventing another order.
- Behavior at the first/last file, and whether navigation wraps.
- Diff presentation (unified or split), handling image changes and unsupported binary files, and navigation across
  staged/unstaged groups when one path appears in both.
- Refresh cadence, retained per-project view preferences, ignored-file visibility, and large-file limits.

## Notes for the developer

**Library candidates**

- [`@pierre/diffs`](https://diffs.com/docs) provides React `File` for ordinary source files and `FileDiff` /
  `PatchDiff` for changes, with Shiki highlighting and virtualization. It is the leading candidate for sharing a
  renderer between file browsing and Git review. Editing support is optional and is outside this requirement.
- [Shiki](https://shiki.style/guide/) is a mature syntax highlighter for mainstream programming languages, with
  lazy-loaded grammars and light/dark themes. It is an alternative highlighting layer, not a complete modal or file
  navigator. Unknown languages can fall back to [plain text](https://shiki.style/languages).
- [`react-diff-view`](https://github.com/otakustay/react-diff-view) remains a candidate for Git patch rendering if
  Pierre does not fit the application. It does not supply the ordinary-file and image viewer together.
- Image rendering is separate from code highlighting. Evaluate additional image-viewer dependencies only if the
  eventual image interactions need them.

**Development notes**

- The UI reaches filesystem and Git capabilities through the daemon protocol, in both browser and desktop clients.
  Use project/worktree identifiers and source-aware reads; do not introduce direct Tauri filesystem access.
- Keep file bodies and patches out of shared reconnect snapshots. Existing request/reply correlation and snapshot
  epochs are available for on-demand reads and reconnect invalidation.
- Current image CSP admits `data:` only; workers, WASM, blob URLs and language chunks need verification with the
  bundled build. Avoid assuming remote CDN assets or widening report-page isolation to accommodate a viewer.
- Projects may be non-Git directories or be registered from a linked worktree. The project's directory is not
  necessarily Git's primary checkout. Define the root and default worktree explicitly at integration time.
- Bound reads and rendering; preserve filenames and binary content without lossy text conversion. Validate paths
  against the selected project/worktree, including symlinks. Render file content as data, without executing repository
  HTML, scripts or embedded SVG markup in the application document.
- Keep existing report ownership and history separate from the project-owned browser. A concurrent plan changes
  report ownership to console-session scope; inspect current implementation before integrating instead of assuming
  either state has shipped.
- Follow existing UI conventions for HeroUI, localization, RTL, keyboard access, focus restoration and narrow panes.
- Each shipped milestone includes its affected checks and product/module documentation updates. This temporary plan
  is not registered in the long-lived documentation index.

**Reference docs**

- `docs/product/window-layout.md`, `docs/product/report-panel.md`, `docs/product/sidebar.md`
- `docs/product/consoles-and-projects.md`, `docs/product/project-git-status.md`
- `packages/ui/README.md`, `apps/daemon/README.md`, `apps/daemon/PROTOCOL.md`
- `docs/memory/writing-ui-components.md`, `docs/memory/writing-daemon-code.md`
- [Git diff semantics](https://git-scm.com/docs/git-diff), [Git worktrees](https://git-scm.com/docs/git-worktree)
