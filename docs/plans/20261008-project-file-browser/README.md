# Project file browser and Git review

## Problem

A project needs a file browser and Git review surface that remain useful without a running session. Its right
pane defaults to a file tree. Opening a file shows source code or an image in a read-only modal; previous/next
controls and left/right arrow keys move through the tree's file order without closing the viewer.

The same project surface shows staged and unstaged changes in a selected local worktree and compares any two local
branches. Project browsing and console-session reports have separate ownership, even when they occupy the same
right-pane space.

## Design decisions

### Sources and scope

- The default tree shows live files, including uncommitted content, under the project's registered directory.
  Reading only committed `HEAD` content would hide the work the user is inspecting while an agent runs.
- The default Git worktree is the checkout containing that directory. It is not whichever checkout happens to
  hold a branch named `main` or `master`, and it may itself be a linked worktree.
- A non-Git project still has file browsing; Git controls show that Git review is unavailable.
- A project associated with a repository subdirectory keeps that scope for both browsing and Git review. The
  daemon discovers the containing repository and records the project's relative directory within it. Selecting
  another worktree for changes maps that same relative scope; it never silently broadens to the whole repository.
- The worktree selector chooses the source of uncommitted changes. It does not check out a branch, change a
  session's working directory or redirect the default Files tree away from the registered project directory.
- Branch comparison means the left branch tip versus the right branch tip, with left as old and right as new.
  Both branches can be selected independently. Merge-base comparison is outside the first version.

### Context and navigation

- Files and Git share an explicit project context. A project can be opened for browsing without creating or
  selecting a session. Browsing state belongs to that project in the client, not to any session or binding.
- Selecting a project session activates its project's browser context. Selecting a console session activates its
  report context. Explicitly opening a project can activate Files/Git while leaving the selected terminal alone;
  the pane identifies its project so the two contexts cannot be confused.
- Report history and form delivery remain separate from browser state. This feature does not move report ownership
  into the project or make the report iframe a file viewer.
- Files is the initial project mode. Viewer navigation consumes the tree's currently visible file rows in
  top-to-bottom order, excluding descendants of collapsed directories. The viewer does not independently sort or
  recursively enumerate the repository. First/last controls are disabled; navigation does not wrap.
- Code and images use the same read-only modal. Unknown text remains inspectable as plain text; unsupported binary
  content gets an explicit presentation. There are no edit/save, stage/unstage, commit or checkout actions.

### Reading and rendering

- The daemon owns filesystem and Git access for browser and desktop clients. Source identity, version information
  and bounded reads are established before the tree and diff surfaces are integrated.
- A file path alone is not an identity: project/repository/worktree, content source and comparison side also matter.
  An accepted diff must not combine a patch from one revision with file bodies from another.
- File bodies and patches are requested on demand, not included in shared reconnect snapshots. Reading, transport,
  parsing and rendering each have limits; UI virtualization cannot substitute for bounded daemon output.
- The application owns a renderer adapter. A short browser and packaged-WebView verification chooses the library
  before the full browser is built, without letting library-specific types define the daemon protocol.

## Milestones

1. 01 Source identities and bounded read contracts (closed)
2. 02 Validate the read-only renderers (closed)
3. 03 Project file browsing and navigation (closed)
4. [Uncommitted changes across worktrees](04-worktree-changes.md) — add scoped worktree selection and staged,
   unstaged and untracked inspection to the usable project pane.
5. [Local branch comparison](05-branch-comparison.md) — compare selected branch tips through the same bounded,
   source-aware review surface.
6. [Final confirmation](06-final-confirmation.md) — checks that could not be run when their milestone closed and that
   nothing later depends on.

Each milestone is independently verifiable and mergeable after its dependencies. Milestones 04 and 05 extend the
working project pane rather than deferring panel integration until after Git review is built.

## Open

These choices do not block the milestone split, but must be resolved before the named milestone is complete:

- **04:** whether diff navigation crosses staged/unstaged groups. A path in both groups always has two distinct
  change identities, regardless of that interaction choice.

## Notes for the developer

**Library candidates**

- [`@pierre/diffs`](https://github.com/pierrecomputer/pierre/tree/main/packages/diffs) is the first candidate to
  evaluate because it renders both ordinary files and diffs. Optional editing features are outside this plan.
- [Shiki](https://shiki.style/guide/) is an alternative highlighting layer, not a complete viewer or navigator.
- [`react-diff-view`](https://github.com/otakustay/react-diff-view) is a fallback patch-rendering candidate if the
  shared renderer does not meet the packaged environment's requirements. Image rendering remains separate.

**Development notes**

- The existing right pane is conditional on a selected console session. Generalize the pane's context through its
  layout, visibility and focus mechanisms, not just by adding tabs inside the report component.
- Report ownership is being changed separately to console-session scope. Inspect the implementation when integrating;
  reuse its then-current ownership and form-routing rules rather than duplicating that migration here.
- The current directory picker lists directories only; it is not a file-tree or file-content API. Likewise, the Git
  badge API supplies branch/upstream metadata, not a changed-file list.
- Keep HeroUI, localization, RTL, keyboard access, focus restoration and narrow-pane behavior consistent with the
  existing UI. Every shipped milestone includes its affected checks and product/module documentation updates.
- This directory is a temporary development plan and is not registered in the long-lived documentation index.

**Reference docs**

- `docs/product/window-layout.md`, `docs/product/report-panel.md`, `docs/product/sidebar.md`
- `docs/product/consoles-and-projects.md`, `docs/product/project-git-status.md`
- `packages/ui/README.md`, `apps/daemon/README.md`, `apps/daemon/PROTOCOL.md`
