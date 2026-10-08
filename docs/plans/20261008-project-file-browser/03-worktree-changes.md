# Uncommitted changes for a selected worktree

> Goal: select a repository worktree and inspect its uncommitted files in staged and unstaged groups.
> Completion criteria: a test repository with linked worktrees shows each worktree's own changes; a file with both
> staged and unstaged edits appears in both groups and each opens the appropriate diff. Untracked files remain
> discoverable as uncommitted files. The selected worktree's changes can be verified independently of the pending
> right-pane routing and default-worktree decision.

## Technical design

- [ ] The project exposes the repository's available local worktrees for selection.
- [ ] An explicitly selected worktree is the source of the uncommitted-file list.
- [ ] Staged and unstaged entries remain distinct even when they have the same file path.
- [ ] Staged review compares the selected worktree's `HEAD` with its index; unstaged review compares its index with
  working-directory content.
- [ ] Opening a change renders its Git diff; changing worktree changes the source of the list and its opened diff.

## Implementation plan

- [ ] Provide daemon-backed worktree discovery and change inspection for an explicitly selected worktree.
- [ ] Present a worktree selector and the staged/unstaged groups over those capabilities.
- [ ] Integrate a read-only diff renderer and source-aware per-file reads with the viewer from
  [milestone 01](01-read-only-viewer.md).
- [ ] Verify different changes in two worktrees, a partially staged file, and untracked, added, deleted and renamed
  entries. Verify unavailable worktrees and detached `HEAD` without showing another checkout's contents.

## Notes for the developer

**Reusable capabilities**

- The daemon Git status module supplies existing project-scoped Git invocation and error-reporting conventions.
- The daemon environment-shell module supplies process deadlines and cleanup.
- The UI daemon-client and store modules supply correlated requests and reconnect epochs.

**Development notes**

- The current project Git status parses only branch/upstream/ahead/behind metadata and excludes untracked files;
  it is not a staged/unstaged change-list API. Its Git runner currently returns lossy UTF-8, so it cannot be reused
  unchanged for binary blobs or NUL-delimited paths.
- This milestone does not choose which worktree "main branch" means; that decision belongs to the pending integration.
- Untracked files have no index baseline. Conflict entries and an unborn `HEAD` also need explicit presentation;
  they must not be silently omitted or mislabeled as ordinary clean comparisons.
- A deleted entry still has an old-side source, and a renamed entry has distinct old/new paths. Index and commit
  content must not be substituted with the live file when opening a diff.
- No checkout is needed to inspect another worktree. Do not couple worktree selection to a session's working directory.
- The default diff layout, image-change rendering, unsupported binary presentation and navigation across groups are
  still open in the overview.

**Reference docs**

- `apps/daemon/README.md`, `apps/daemon/PROTOCOL.md`
- `docs/product/project-git-status.md`
- `docs/memory/writing-daemon-code.md`
- [Git diff](https://git-scm.com/docs/git-diff), [Git worktree](https://git-scm.com/docs/git-worktree)
