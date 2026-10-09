# Uncommitted changes across worktrees

> Goal: inspect each local worktree's uncommitted changes in the existing project browser.
> Completion criteria: two worktrees with different edits show their own scoped changes. A partially staged file
> appears in both groups with the correct diff, untracked files remain discoverable, and deleted/renamed entries
> open their correct sources. Concurrent staging/committing and unavailable worktrees produce coherent results or
> explicit stale/unavailable states. The complete flow works through the real project pane.

It extends the shipped project pane (`docs/product/project-pane.md`; code in `packages/ui/src/browser/`, see
`packages/ui/README.md`); all reads follow the browse contract in `apps/daemon/PROTOCOL.md`'s "Browsing a project".

## Technical design

- [ ] The project Git mode discovers available local worktrees and identifies their checkout paths and branch or
  detached-HEAD state. Initial selection and subdirectory mapping follow the overview.
- [ ] Selecting a worktree changes the uncommitted-change source and invalidates the old list and opened diff. An
  unavailable checkout is not replaced with another checkout's content, and no checkout operation is performed.
- [ ] Staged entries compare `HEAD` with index content; unstaged entries compare index with live working content.
  Group is part of entry identity, so one path can appear in both without sharing a baseline or viewer cache entry.
- [ ] Untracked files have no index baseline and are explicitly identified among uncommitted changes. They open as
  new content, not as a fabricated index comparison. Ignored-file treatment is consistent with the chosen scope rules.
- [ ] Added/deleted sides and renamed old/new paths remain explicit. For a project subdirectory, a change belongs
  to its review when either side is in scope. A rename crossing the boundary shows path metadata and a restricted
  preview of the in-scope side; the out-of-scope body is not read or returned, including through patch hunks.
  Label the restricted side instead of presenting a complete rename patch or an ordinary addition/deletion diff.
- [ ] An unborn `HEAD` has an explicit empty baseline for staged additions. Conflict entries have an explicit
  conflicted presentation and do not masquerade as ordinary two-sided diffs. Unsupported change types stay visible
  with a reason even when their content cannot be rendered.
- [ ] Patch and content sources follow the version contract. A change to the index, `HEAD` or live file between
  listing and opening causes a coherent reread or stale result, never a mixture of versions or a live-file fallback
  for an index/commit side.
- [ ] The tested diff adapter renders text and the supported image-change presentation; unsupported binaries and
  oversized changes retain their metadata and explicit fallback. Any change navigation preserves group identity.

## Implementation plan

- [ ] Connect worktree discovery, scoped change listings and per-change inspection to the controlled Git runner
  and bounded source contracts, with distinct list and content requests.
- [ ] Add the Git mode, worktree selector and change groups to the existing project pane, preserving its per-project
  state and refresh/invalidation behavior.
- [ ] Verify different edits in two worktrees, detached `HEAD`, an unborn branch, a partially staged file, untracked
  content, additions, deletions, renames and conflicts through the integrated viewer.
- [ ] Verify a project subdirectory in another worktree, missing live subdirectories, deletion of an entire scoped
  directory and modified renames crossing its boundary. Their replies and previews must contain no out-of-scope
  body content. Missing live files do not hide in-scope index/commit deletions.
- [ ] Verify a worktree removed or replaced after discovery and rapid worktree changes with delayed replies.
  Selecting unavailable sources must leave an explicit result rather than inspect a different checkout.
- [ ] Verify staging, committing and overwriting a file after a listing and between content reads. Patch metadata
  and displayed bodies must identify the same accepted versions.
- [ ] Verify large change sets, oversized patches and overlapping refreshes against declared budgets. Run affected
  checks and update the shipped protocol, product and module documentation.

## Handoff

- Serve and verify the change identity recorded as a contract ahead of its requests in `apps/daemon/PROTOCOL.md`'s
  "Changes and comparisons": group, old/new sides with `present` / `absent` / `out_of_scope` states, side `kind`
  (`file` / `symlink` / `submodule`, a type change being one change whose sides differ), the patch and change-entry
  budgets with their `patch_bytes` / `change_entries` limits (declared there, no Rust constant yet), and the diff
  reply's own worst-case reservation. Adjust the contract where serving it proves it wrong.
- `git diff` writes a file↔symlink type change as two file sections in one patch, which the viewer's diff library
  renders as one combined change. Check this against a real type change and present it as the contract's single
  change whose sides differ in `kind`.
- The project pane's mode lives in `BrowserMode` (`packages/ui/src/browser/browserState.ts`, persisted per project);
  only `files` exists. Add the Git mode there and its switch in the `ProjectBrowser` header; the aside's owner stays
  the project, and its labels (`ASIDE_LABELS` in `layout/asideOwner.ts`, "project pane") already fit a Git view.
- `get_project_source` is not called by the UI yet; Git availability and the worktree list come from it.
- The viewer's content key names its source (`live`); index, commit and branch reads need keys of their own, with the
  change group part of the identity.
- `neighbours()` in `packages/ui/src/browser/tree.ts` is the ordering rule for viewer navigation; reuse its approach
  for change lists rather than letting the viewer sort.
- Listing requests are bounded per browser and per window (`useDirectoryListings.ts`); Git list and content requests
  share the daemon's per-connection bound and need to be sized together with them.

## Notes for the developer

**Reusable capabilities**

- The daemon Git status module supplies project-scoped Git error conventions; the environment-shell module supplies
  deadlines and cleanup. The source foundation supplies the required controlled, byte-preserving reads.
- The project browser supplies pane ownership, bounded request handling and the viewer adapter.

**Development notes**

- The existing Git badge parser excludes untracked files and only reads branch/upstream metadata. It is not a
  staged/unstaged API and its refresh operation may fetch or fast-forward; do not use that operation to implement
  read-only change inspection.
- Automatic fast-forward and agent Git commands can change the source while the viewer is open. The source
  foundation's connection epochs and Git content versions solve different invalidation problems.
- Git metadata can still describe deleted paths when no corresponding live directory exists. Historical/index
  reads must not depend on successfully opening a live counterpart.

**Reference docs**

- `apps/daemon/README.md`, `apps/daemon/PROTOCOL.md`, `packages/ui/README.md`
- `docs/product/project-git-status.md`
- [Git diff](https://git-scm.com/docs/git-diff), [Git worktree](https://git-scm.com/docs/git-worktree)
